import {test,expect} from '@playwright/test';

test('profile endpoint stays distinct from defaults until explicitly overridden',async({page}, testInfo)=>{
  await page.goto('/?panel=settings&section=network&mode=precedence');
  const panel=page.getByRole('region',{name:'配置优先级'});
  await expect(panel.getByText('127.0.0.1:7891',{exact:true})).toBeVisible();
  await expect(page.getByRole('textbox',{name:'代理监听端口'})).toHaveValue('7890');
  const choice=panel.getByRole('switch',{name:'代理入口客户端覆盖'});
  await expect(choice).not.toBeChecked();
  await choice.click();
  await panel.getByRole('button',{name:'保存优先级'}).click();
  await expect(panel.getByText('127.0.0.1:7890',{exact:true})).toBeVisible();
  await expect(panel.getByText('客户端显式覆盖',{exact:true})).toBeVisible();
  await expect(panel.getByRole('switch',{name:'DNS客户端覆盖'})).not.toBeChecked();
  await choice.click();
  await panel.getByRole('button',{name:'保存优先级'}).click();
  await expect(panel.getByText('127.0.0.1:7891',{exact:true})).toBeVisible();
  await panel.screenshot({path: testInfo.outputPath("precedence-desktop.png")});
  await page.setViewportSize({width:390,height:844});
  expect(await panel.evaluate(el=>el.scrollWidth<=el.clientWidth+1)).toBe(true);
  await panel.screenshot({path: testInfo.outputPath("precedence-mobile.png")});
});
